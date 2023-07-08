import { Injectable } from '@nestjs/common'
import { InjectModel } from '@nestjs/mongoose'
import { Model } from 'mongoose'
import { FusionModel } from '@/db/schemas/fusion.schema'
import { FusionDto } from '@server-octopus/types'

@Injectable()
export class MongoService {
    constructor(
        @InjectModel(FusionModel.name) private fusionModel: Model<FusionModel>
    ) {}

    async addFusion(fusion: FusionDto) {
        const createdFusion = new this.fusionModel(fusion)
        return createdFusion.save()
    }
}
