import { Injectable } from '@nestjs/common';
import { CreateProfileDto } from './dto/create-profile.dto';
import { UpdateProfileDto } from './dto/update-profile.dto';
import {PrismaService} from "@/db/prisma.service";
import {ProfileVo} from "@/profile/vo/profile.vo";
import {UserService} from "@/user/user.service";

@Injectable()
export class ProfileService {

  constructor(private readonly prisma: PrismaService, private readonly userService: UserService) {}

  async getOneByUserId(userId: string) {
    const val =  await this.prisma.profile.findUnique({
        where: {
            user_id: userId
        }
    })
    return {
      name: val?.name,
      avatar: val?.avatar,
      description: val?.description,
      user: await this.userService.getOneByUserId(userId)
    } as ProfileVo
  }
  create(createProfileDto: CreateProfileDto) {
    return 'This action adds a new profile';
  }

  findAll() {
    return `This action returns all profile`;
  }

  findOne(id: number) {
    return `This action returns a #${id} profile`;
  }

  update(id: number, updateProfileDto: UpdateProfileDto) {
    return `This action updates a #${id} profile`;
  }

  remove(id: number) {
    return `This action removes a #${id} profile`;
  }
}
