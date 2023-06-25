import {Injectable, Logger, UnauthorizedException} from '@nestjs/common';
import { UserService } from '@/user/user.service';
import { JwtService } from '@nestjs/jwt';
import * as bcrypt from 'bcrypt';
import {ConfigService} from "@nestjs/config";

@Injectable()
export class AuthService {
  private readonly logger = new Logger(AuthService.name);

  constructor(
    private usersService: UserService,
    private jwtService: JwtService,
    private configService: ConfigService
  ) {}

  async signIn(pass: string, email?: string, username?: string) {
    if (!email && !username) {
      this.logger.error('email or username is required')
      throw new UnauthorizedException();
    }
    const user = await this.usersService.user(email ? { email } : { username });
    this.logger.log(`user: ${JSON.stringify(user)}`)
    if (!user || !(await bcrypt.compare(pass, user.password))) {
        this.logger.warn(`user not found or password is incorrect`)
      throw new UnauthorizedException();
    }
    const payload = { userId: user.user_id };
    return {
      access_token: await this.jwtService.signAsync(payload),
      refresh_token: await this.jwtService.signAsync(payload)
    };
  }

  async register(pass: string, email: string, username: string) {
    const user = await this.usersService.createUser({
      email,
      password: pass,
      username,
    });
  }

  getSaltRounds() {
    const saltRounds = this.configService.get('SALT_ROUNDS')
    return saltRounds ? parseInt(saltRounds) : 10
  }

  /**
   * unit seconds from .env
   */
  getJwtAccessExpirationTime() {
    const jwtExpirationTime = this.configService.get('JWT_EXPIRATION_TIME')
    return jwtExpirationTime ? parseInt(jwtExpirationTime) : 86400
  }

  /**
   * unit seconds from .env
   */
  getJwtRefreshExpirationTime() {
    const jwtRefreshExpirationTime = this.configService.get('JWT_REFRESH_EXPIRATION_TIME')
    return jwtRefreshExpirationTime ? parseInt(jwtRefreshExpirationTime) : 86400 * 6
  }
}
